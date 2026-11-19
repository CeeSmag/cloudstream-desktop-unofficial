//! Which background answer is still wanted.
//!
//! Home, search, Browse, a title page and the opening of a video run on worker threads and can be
//! replaced while they run: the viewer picks another provider, types a new search, clicks a second
//! title, or opens another video. Without a check, the older answer could land last and win (an old
//! title page reopening after it was closed, the first of two opened videos starting to play).
//! Every request gets a ticket; an answer is shown only while its ticket is the newest of its kind.

/// The kinds of request that replace each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestKind {
    Home,
    Search,
    Browse,
    Details,
    Open,
    /// Online skip times of the playing stream.
    Skips,
}

/// Names one request; answers carry it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ticket {
    kind: RequestKind,
    generation: u64,
}

impl Ticket {
    /// The kind of request this ticket belongs to.
    pub(crate) fn kind(self) -> RequestKind {
        self.kind
    }
}

/// The newest generation of each kind.
#[derive(Debug, Default)]
pub(crate) struct RequestTickets {
    home: u64,
    search: u64,
    browse: u64,
    details: u64,
    open: u64,
    skips: u64,
}

impl RequestTickets {
    fn generation_mut(&mut self, kind: RequestKind) -> &mut u64 {
        match kind {
            RequestKind::Home => &mut self.home,
            RequestKind::Search => &mut self.search,
            RequestKind::Browse => &mut self.browse,
            RequestKind::Details => &mut self.details,
            RequestKind::Open => &mut self.open,
            RequestKind::Skips => &mut self.skips,
        }
    }

    fn generation(&self, kind: RequestKind) -> u64 {
        match kind {
            RequestKind::Home => self.home,
            RequestKind::Search => self.search,
            RequestKind::Browse => self.browse,
            RequestKind::Details => self.details,
            RequestKind::Open => self.open,
            RequestKind::Skips => self.skips,
        }
    }

    /// A new request of `kind`: every earlier ticket of that kind is stale from now on.
    pub(crate) fn issue(&mut self, kind: RequestKind) -> Ticket {
        let generation = self.generation_mut(kind);
        *generation += 1;
        Ticket {
            kind,
            generation: *generation,
        }
    }

    /// The viewer abandoned `kind` (closed the page or the player): no earlier answer is shown.
    pub(crate) fn cancel(&mut self, kind: RequestKind) {
        *self.generation_mut(kind) += 1;
    }

    /// The ticket of the request of `kind` that is still wanted, if one runs.
    pub(crate) fn current(&self, kind: RequestKind) -> Ticket {
        Ticket {
            kind,
            generation: self.generation(kind),
        }
    }

    /// True when `ticket`'s answer is still wanted.
    pub(crate) fn is_current(&self, ticket: Ticket) -> bool {
        self.generation(ticket.kind) == ticket.generation
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn is_current_newer_request_makes_older_stale() {
        let mut tickets = RequestTickets::default();
        let first = tickets.issue(RequestKind::Details);
        let second = tickets.issue(RequestKind::Details);
        assert!(!tickets.is_current(first));
        assert!(tickets.is_current(second));
    }

    #[test]
    fn is_current_kinds_do_not_affect_each_other() {
        let mut tickets = RequestTickets::default();
        let home = tickets.issue(RequestKind::Home);
        tickets.issue(RequestKind::Search);
        tickets.cancel(RequestKind::Open);
        assert!(tickets.is_current(home));
    }

    #[test]
    fn is_current_skip_times_of_a_replaced_stream_are_dropped() {
        // Stream A's lookup is slow; the viewer switches to stream B, whose lookup starts.
        let mut tickets = RequestTickets::default();
        let stream_a = tickets.issue(RequestKind::Skips);
        let stream_b = tickets.issue(RequestKind::Skips);
        tickets.issue(RequestKind::Open);
        assert!(!tickets.is_current(stream_a), "A's late answer is dropped");
        assert!(
            tickets.is_current(stream_b),
            "opening steps do not touch skip lookups"
        );
    }

    #[test]
    fn cancel_closed_page_answer_arriving_late_is_dropped() {
        // Title page opened, viewer closes it, the slow answer arrives afterwards.
        let mut tickets = RequestTickets::default();
        let opened = tickets.issue(RequestKind::Details);
        tickets.cancel(RequestKind::Details);
        assert!(!tickets.is_current(opened));
    }

    #[test]
    fn current_matches_last_issued() {
        let mut tickets = RequestTickets::default();
        let open = tickets.issue(RequestKind::Open);
        assert_eq!(tickets.current(RequestKind::Open), open);
        assert_eq!(open.kind(), RequestKind::Open);
    }

    #[test]
    fn is_current_answers_in_any_order_only_newest_request_is_shown() {
        // Workers answer 200 searches in a scrambled order; the screen must end on the last one.
        let mut tickets = RequestTickets::default();
        let issued: Vec<(Ticket, usize)> = (0..200)
            .map(|query| (tickets.issue(RequestKind::Search), query))
            .collect();
        let (answers, received) = mpsc::channel();
        let workers: Vec<_> = issued
            .chunks(25)
            .map(|chunk| {
                let (chunk, answers) = (chunk.to_vec(), answers.clone());
                std::thread::spawn(move || {
                    for (ticket, query) in chunk.into_iter().rev() {
                        answers.send((ticket, query)).expect("send");
                    }
                })
            })
            .collect();
        drop(answers);
        for worker in workers {
            worker.join().expect("worker");
        }
        let mut shown = None;
        while let Ok((ticket, query)) = received.recv_timeout(Duration::from_secs(1)) {
            if tickets.is_current(ticket) {
                assert!(shown.is_none(), "a second answer was shown");
                shown = Some(query);
            }
        }
        assert_eq!(shown, Some(199));
    }
}

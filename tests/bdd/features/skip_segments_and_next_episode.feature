Feature: Skip-segment merging and skip / next-episode prompts

  Scenario: Chapters take precedence over online skip times and drive skip and next-episode prompts
    Given a clean app data directory with a local SQLite database
    And an episode of length 1440000 ms with chapter "Opening" from 30000 to 120000 ms
    And online AniSkip returns intro 10000 to 100000 ms and credits 1350000 to 1435000 ms
    When chapter segments and online skip segments are merged
    Then the merged intro uses the chapter timing 30000 to 120000 ms
    And the merged credits use the online timing 1350000 to 1435000 ms
    And at position 60000 ms the prompt offers "Skip Intro" seeking to 120000 ms
    And at position 1380000 ms with a next episode the prompt offers "Next episode"

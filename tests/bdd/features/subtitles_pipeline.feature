Feature: Subtitle prefetch, normalization, and language preference selection

  Scenario: Prefetch prefers user language, normalizes gzip/UTF-16, and rejects HTML error pages
    Given a clean app data directory with a local SQLite database
    And external subtitle tracks for French, gzipped English, UTF-16 Arabic, and an HTML 403 page
    When the user prefetches subtitles preferring "English" then "Arabic"
    Then the HTML error page subtitle is rejected
    And the English and Arabic subtitles are normalized to UTF-8 local files on disk
    And the highest priority delivered subtitle language is "English"

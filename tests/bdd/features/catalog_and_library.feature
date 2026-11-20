Feature: Catalog discovery, details, bookmarks, and continue watching

  Scenario: Browse home, search title, inspect details, bookmark, and resume
    Given a clean app data directory with a local SQLite database
    And provider "CineStream" has home and search title "Severance" with episode "ep-s1e1"
    When the user loads the home page and searches for "Severance" on "CineStream"
    And the user opens details for "Severance"
    And the user bookmarks "Severance"
    And the user watches "Severance" episode "ep-s1e1" up to 180000 ms of 3600000 ms and stops
    Then "Severance" is bookmarked in the library
    And continue watching lists "Severance" resuming at 180000 ms

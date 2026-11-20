Feature: Extension repository and plugin lifecycle

  Scenario: Add repository, install plugin, toggle trust, and uninstall
    Given a clean app data directory with a local SQLite database
    And a remote repository "MainRepo" offering plugin "CineStream" providing "CineStreamProvider"
    When the user adds the repository "MainRepo"
    And the user installs plugin "CineStream" from "MainRepo"
    Then provider "CineStreamProvider" is discovered and untrusted
    When the user marks plugin "CineStream" as trusted
    Then plugin "CineStream" is recorded as trusted and reloaded in trusted mode
    When the user marks plugin "CineStream" as untrusted
    Then plugin "CineStream" is recorded as untrusted and reloaded in sandboxed mode
    When the user uninstalls plugin "CineStream"
    Then provider "CineStreamProvider" is no longer offered and no extensions remain installed

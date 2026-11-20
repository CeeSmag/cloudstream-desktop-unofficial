Feature: Stream resolution, probing, fallback, and watch progress

  Scenario: Fall back from broken high-quality stream to best working stream and save progress
    Given a clean app data directory with a local SQLite database
    And a local stream server with broken 1080p stream and working 720p stream
    When the user opens streams for "Dune" on "CineStream"
    Then playback picks the working 720p stream
    When playback advances to 90000 ms of 600000 ms and stops
    Then watch progress for "Dune" is saved at 90000 ms

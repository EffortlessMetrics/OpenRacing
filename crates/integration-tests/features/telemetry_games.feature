Feature: Game Telemetry Auto-Detection
  As a sim racer
  I want my force feedback software to automatically detect the running game
  So that the correct telemetry adapter is loaded without manual configuration

  Scenario Outline: Game process is detected and telemetry starts
    Given the process "<process>" is running
    When the game detection service scans running processes
    Then telemetry is started for game "<game_id>"
    And the appropriate adapter is loaded

    Examples:
      | process                        | game_id      |
      | iRacingSim64.exe               | iracing      |
      | dirtrally2.exe                 | dirt_rally_2 |
      | AC2-Win64-Shipping.exe         | acc          |

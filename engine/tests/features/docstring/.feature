Feature: Docstring
  Scenario: Docstring delivery
    Given a step with words
      """
      hello docstring
      """
    Then the words are seen

  Scenario: Datatable delivery
    Given a step with rows
      | alpha | beta |
      | gamma | delta  |
    Then the rows are seen

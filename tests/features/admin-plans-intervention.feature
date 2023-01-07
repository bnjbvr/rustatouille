# As a service administrator,
# I want to be able to plan a new intervention
# So that users are warned about a possible downtime
Feature: Plan a new intervention

    Background:
        Given there is a service named "Framasphère" with URL "https://framasphere.org"
        And there is a service named "Framaspace" with URL "https://frama.space"

    Scenario:
        When I go to the admin page entering the password
        Then the title of the page should be "Administration"
        And "Planed intervention" should be empty
        When I click on "Plan a new intervention"
        Then the title of the page should be "New intervention"
        When I fill the "Title" with "Mise à jour du noyau"
        And I fill the "Planned start date" field with "2025-02-13T08:00:00"
        And I fill the "Estimated end date" field with "2025-02-13T08:15:00"
        And I fill the "Description" with "Rédemarrage pour mettre à jour le noyau"
        And I select "Interruption totale" within "Severity"
        And I submit the form
        Then the title of the page should be "Administration"
        And I should see a confirmation message "The intervention \"Mise à jour du noyau\" has been planed"
        

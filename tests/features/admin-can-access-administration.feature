# As a service administrator
# I want to be the only one accessing the administration
# So that I can manage the interventions securely
Feature: The administration is protected

    Scenario:
        When I go to the admin page
        Then I should see a field "Password required"
        When I fill "Password required" with "aaaa"
        And I submit the form
        Then I should see an error message "Password incorrect"
        When I fill the good admin password
        Then the title of the page should be "Administration"
# As a service administrator
# I want to be able to change the intervention status
# So that I can keep users updated
Feature: Change intervention status

    Background:
        Given there is a service named "Framaspace" with URL "https://frama.space"
        And there is an intervention planed with title "Redémarrage du serveur" 

    Scenario: The intervention start
        When 

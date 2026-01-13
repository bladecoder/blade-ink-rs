I looked...
        * [at the door]
          -> door_open
        * [outside]
          -> leave
        
        === door_open
        at the door. It was open.
        -> leave
        
        === leave
        I stood up and...        
        { door_open:
            *   I strode out of the compartment[] and I fancied I heard my master quietly tutting to himself. -> END         
        - else:
            *   I asked permission to leave[] and Monsieur Fogg looked surprised.    -> END  
            *   I stood and went to open the door[]. Monsieur Fogg seemed untroubled by this small rebellion. -> END  
        }
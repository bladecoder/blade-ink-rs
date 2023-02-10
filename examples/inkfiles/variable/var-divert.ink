VAR current_epilogue = -> everybody_dies
        Divert as variable example
        -> continue_or_quit
        
        === continue_or_quit
        Give up now, or keep trying to save your Kingdom?
        *  [Keep trying!]   -> continue_or_quit
        *  [Give up]        -> current_epilogue
        
        === everybody_dies
        Everybody dies.
        -> END
config -1> executor -2> solver -3> output              -4> ingest -5> database
                               -3> runtime + exit code -4> ingest |
1. SolverConfig
2. Paramters as collection of OsStrings
3. Output - stdout: String
          - stderr: String
	  - absolute runtime
	  - exit code

4. TestMetrics - runtime
               - parse time
               - satisfibility
	       - memory used
	       - restarts
	       - conflicts
	       - propagations
	       - conflict literals
	       - number of variables
	       - number of clauses

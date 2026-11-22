#include "side.h"
// spec/digest.md §1 rule 2: typedef chain does not matter.
#ifdef B
typedef int T1; typedef T1 T2; typedef T2 T3; T3 v;
#else
typedef int T1; typedef T1 T2; typedef T2 T3; int v;
#endif

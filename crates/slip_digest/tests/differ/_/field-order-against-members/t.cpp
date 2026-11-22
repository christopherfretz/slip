#include "side.h"
// digest §5, §5.5 RecordData `members-fields`: fields swapped across a member function.
#ifdef B
struct C { int b; void f(); int a; };
#else
struct C { int a; void f(); int b; };
#endif

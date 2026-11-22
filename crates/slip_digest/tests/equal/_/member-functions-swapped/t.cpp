#include "side.h"
// digest §5, §5.5 RecordData `members-other`: the order of member functions is not behaviour.
#ifdef B
struct C { void g(); void f(); };
#else
struct C { void f(); void g(); };
#endif

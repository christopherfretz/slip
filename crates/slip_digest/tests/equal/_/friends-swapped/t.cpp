#include "side.h"
// digest §5, §5.5 RecordData `members-other`: the order of friends is not behaviour.
#ifdef B
void f(); void g(); struct C { friend void g(); friend void f(); };
#else
void f(); void g(); struct C { friend void f(); friend void g(); };
#endif

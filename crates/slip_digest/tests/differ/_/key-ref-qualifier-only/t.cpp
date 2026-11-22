#include "side.h"
// digest §3.4, WP-039 item 1: member overloads differing only in ref-qualifier: two keys.
struct S { void f() &;
#ifdef B
void f() &&;
#endif
};

#include "side.h"
// digest §3.4, WP-039 item 1: member overloads differing only in cv-qualifier: two keys.
struct S { void f() const;
#ifdef B
void f() volatile;
#endif
};

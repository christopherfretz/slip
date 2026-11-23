#include "side.h"
// digest §3.4, WP-039 item 1: unnamed enumerations in a class and a namespace: two keys.
struct S { enum { A }; };
#ifdef B
enum { A2 };
#endif

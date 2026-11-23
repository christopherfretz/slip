#include "side.h"
// digest §3.4, WP-039 item 1: deduction guides of two signatures: two keys.
template <class T> struct S { S(T); };
S(int) -> S<long>;
#ifdef B
S(char) -> S<long>;
#endif

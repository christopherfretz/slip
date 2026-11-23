#include "side.h"
// digest §3.4, WP-039 item 1: a function and a function template of one signature: two keys.
void f(int);
#ifdef B
template <class T> void f(int);
#endif

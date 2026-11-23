#include "side.h"
// digest §3.4, WP-039 item 1: two literal operators: two keys.
int operator""_a(unsigned long long);
#ifdef B
int operator""_b(unsigned long long);
#endif

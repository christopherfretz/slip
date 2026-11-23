#include "side.h"
// WP-034 (§6): two statements swapped.
#ifdef B
void f(int &x) { x = 2; x = 1; }
#else
void f(int &x) { x = 1; x = 2; }
#endif

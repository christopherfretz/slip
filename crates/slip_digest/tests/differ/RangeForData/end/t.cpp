#include "side.h"
// §6 RangeForData `end`: another `end` is selected.
#ifdef B
struct Q {};
int *begin(Q &);
int *end(const Q &);
int *end(Q &);
void f(Q &q) { for (int e : q) ; }
#else
struct Q {};
int *begin(Q &);
int *end(const Q &);
void f(Q &q) { for (int e : q) ; }
#endif

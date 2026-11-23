#include "side.h"
// §6 RangeForData `begin`: another `begin` is selected; the range and the rest are the same.
#ifdef B
struct Q {};
int *begin(const Q &);
int *end(Q &);
int *begin(Q &);
void f(Q &q) { for (int e : q) ; }
#else
struct Q {};
int *begin(const Q &);
int *end(Q &);
void f(Q &q) { for (int e : q) ; }
#endif

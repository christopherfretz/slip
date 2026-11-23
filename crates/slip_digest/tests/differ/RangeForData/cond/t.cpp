#include "side.h"
// §6 RangeForData `cond`: another `!=` is selected.
struct It {};
struct R { It begin(); It end(); };
bool operator!=(const It &, const It &);
void operator++(const It &);
int operator*(const It &);
#ifdef B
bool operator!=(It &, It &);
void f(R &r) { for (int e : r) ; }
#else
void f(R &r) { for (int e : r) ; }
#endif

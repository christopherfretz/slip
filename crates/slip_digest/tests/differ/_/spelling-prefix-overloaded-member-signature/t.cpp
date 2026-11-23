#include "side.h"
// §4.4 "Spellings", rule 11, F1: the same qualifier in a trailing return type (visible to SFINAE): profile mode raises the mark and rule 1 reads it: side B adds `f(long)`, which the found set of `f` there does not hold.
template<int N> struct Bk { static int k(int); static int k(double); };
struct DD : Bk<8> {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t, DD x) -> decltype(x.Bk<sizeof(sizeof(f(t)))>::k(t));

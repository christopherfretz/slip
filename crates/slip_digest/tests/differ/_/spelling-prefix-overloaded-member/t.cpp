#include "side.h"
// §4.4 "Spellings", rule 11, F1 of the re-check of fix round 3: an overloaded member's qualifier in a body (`x.Bk<..>::k(t)`, `member_overload`), written by rule 11: side B adds `f(long)`, which the found set of `f` there does not hold.
template<int N> struct Bk { static int k(int); static int k(double); };
struct DD : Bk<8> {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t, DD x) { return x.Bk<sizeof(sizeof(f(t)))>::k(t); }

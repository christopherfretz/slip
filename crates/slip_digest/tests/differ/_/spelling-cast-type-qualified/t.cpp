#include "side.h"
// §4.4 "Spellings", rule 1: a cast's type is read as written (`cast_written`), not as the node's: the prvalue of `static_cast<const decltype(sizeof(f(T())))>` has neither the qualifier nor the sugar (it is `unsigned long`), while as written it holds the found set of `f`.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(static_cast<const decltype(sizeof(f(T())))>(0) + t);

#include "side.h"
// §4.4 "Spellings": a found set in the type of a cast as written.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(static_cast<decltype(f(t))>(0));

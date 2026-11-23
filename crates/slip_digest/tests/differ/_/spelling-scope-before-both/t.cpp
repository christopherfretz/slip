#include "side.h"
// §4.4 "Spellings": f(long) added before both g and h is in both found sets.
void f(int);
#ifdef B
void f(long);
#endif
template<class T> auto g(T t) -> decltype(f(t));
template<class T> auto h(T t) -> decltype(f(t));

#include "side.h"
// §4.4 "Spellings", the example of WP-041: g and h have one return type by profile; f(long) added between them is in h's found set only.
void f(int);
template<class T> auto g(T t) -> decltype(f(t));
#ifdef B
void f(long);
#endif
template<class T> auto h(T t) -> decltype(f(t));

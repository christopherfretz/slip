#include "side.h"
// §4.4 "Spellings": the first spelling of the profile (g's) found nothing, h's own spelling finds f(long) on side B. The canonical type is not marked; h's list is its own.
template<class T> auto g(T t) -> decltype(f(t));
void f(int);
#ifdef B
void f(long);
#endif
template<class T> auto h(T t) -> decltype(f(t));

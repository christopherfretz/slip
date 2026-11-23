#include "side.h"
// §4.4 "Spellings": the allocation function a resolved placement new chose where it was written is not in the profile; on side B a better one is declared between g and h.
using size_t = decltype(sizeof 0);
void* operator new(size_t, long);
template<class T> auto g(T t) -> decltype(new (1) int, t);
#ifdef B
void* operator new(size_t, int);
#endif
template<class T> auto h(T t) -> decltype(new (1) int, t);

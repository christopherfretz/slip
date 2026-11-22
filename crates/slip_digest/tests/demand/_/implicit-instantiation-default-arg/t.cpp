#include "side.h"
// §11.4: a default argument of a member of an implicit instantiation is not written; a call instantiates it.
template <class T> struct Q { void g(int a = sizeof(T)); };
Q<int> q;
#ifdef B
void h() { q.g(); }
#else

#endif

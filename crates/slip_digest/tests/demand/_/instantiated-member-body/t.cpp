#include "side.h"
// §11.9: the body of a member of an implicit instantiation is the resolved unit `definition`; a use that instantiates it changes no other unit.
template <class T> struct Q { void g() {} };
Q<int> q;
#ifdef B
void h() { q.g(); }
#else

#endif

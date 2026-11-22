#include "side.h"
// §11.9: the initialiser of a static data member of an implicit instantiation, and its `0-inline`, are the resolved unit `definition`'s; a use that instantiates them changes no unit.
template <class T> struct Q { static inline int s = 1; };
Q<int> q;
#ifdef B
int h() { return Q<int>::s; }
#else

#endif

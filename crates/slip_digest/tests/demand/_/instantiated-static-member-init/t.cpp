#include "side.h"
// §11.9: the initialiser of a static data member of an implicit instantiation.
template <class T> struct Q { static inline int s = 1; };
Q<int>* p;
#ifdef B
int h() { return Q<int>::s; }
#else

#endif

#include "side.h"
// §11.9 (WP-026): a member of a class named by an explicit instantiation declaration is defined only when used.
template <class T> struct Q { void g() {} static inline int s = 1; };
extern template struct Q<int>;
#ifdef B
void h() { Q<int>().g(); }
int y() { return Q<int>::s; }
#else

#endif

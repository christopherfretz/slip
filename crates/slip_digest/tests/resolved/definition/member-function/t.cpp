#include "side.h"
// digest-kinds §11.9: the body of a member function of an implicit
// instantiation is its resolved unit `definition`, made by the use.
template <class T> struct Q { int g() { return 1; } };
Q<int> q;
#ifdef B
int h() { return q.g(); }
#endif

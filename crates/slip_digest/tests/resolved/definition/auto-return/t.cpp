#include "side.h"
// digest-kinds §11.9: the return type of a member function of an implicit
// instantiation whose pattern declares `auto` is deduced with the body,
// and is `definition`'s with `0-return-deduced`.
template <class T> struct Q { auto g() { return 1; } };
Q<int> q;
#ifdef B
int h() { return q.g(); }
#endif

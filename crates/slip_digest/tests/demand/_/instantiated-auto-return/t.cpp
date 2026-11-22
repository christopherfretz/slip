#include "side.h"
// §11.9 FnSpec `0-return-deduced`: the deduced return of a member of an implicit instantiation is the definition's.
template <class T> struct Q { auto g() { return 1; } };
Q<int> q;
#ifdef B
int h() { return q.g(); }
#else

#endif

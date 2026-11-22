#include "side.h"
// §5.4 ParamData `flags-default-arg-pending`, §11.4: a call instantiates the default argument of a member of a class template specialisation.
template <class T> struct Q { void g(T a = T()); };
Q<int> q;
#ifdef B
void h() { q.g(); }
#else

#endif

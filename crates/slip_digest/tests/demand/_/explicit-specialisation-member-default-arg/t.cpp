#include "side.h"
// §11.4 (WP-026): a member explicitly specialised keeps the declaration Kiln instantiated in its chain; the default Kiln instantiates there on use is not the unit's.
template <class T> struct Q { void g(int a = sizeof(T)); };
template <> void Q<int>::g(int a) {}
#ifdef B
void h() { Q<int>().g(); }
#else

#endif

#include "side.h"
// digest-kinds §11.3: a variable template's specialisation is evaluated
// when Kiln instantiates it, on use.
template <class T> constexpr int v = sizeof(T);
#ifdef B
int h() { int x[v<int>]; return sizeof x; }
#endif

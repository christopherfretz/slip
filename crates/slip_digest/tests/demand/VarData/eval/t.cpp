#include "side.h"
// §5.4 VarData eval, §11.3: a use of `v<int>` as a value added on side B changes no unit; eval is not written.
template <class T> constexpr int v = sizeof(T);
const int* p = &v<int>;
#ifdef B
int y = v<int>;
#else

#endif

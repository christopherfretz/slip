#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> int f() { if constexpr (sizeof(T) == 4) return X; else return 0; }

#include "side.h"
// §5.5 BaseSpec `is_virtual`.
#ifdef B
struct P {}; struct C : virtual P {};
#else
struct P {}; struct C : P {};
#endif

#include "side.h"
// WP-035, digest-kinds §7 `ConstructInfo` `list`.
struct S { S(int); S(long); };
S f();
#ifdef B
S v{1};
#else
S v(1);
#endif

#include "side.h"
// §5.6 FriendTarget `Type.owned_tag` (out): a `friend struct H;` that declares `H` against one that names an `H` declared before; `H` is a unit of its own either way.
#ifdef B
struct H;
struct X { friend struct H; };
#else
struct X { friend struct H; };
struct H;
#endif

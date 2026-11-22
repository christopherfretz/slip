#include "side.h"
// §5.5 BaseSpec `access` (landed WP-027): a base written without an access
// specifier is private under `class`, public under `struct`.
struct P {};
#ifdef B
class C : P {};
#else
struct C : P {};
#endif

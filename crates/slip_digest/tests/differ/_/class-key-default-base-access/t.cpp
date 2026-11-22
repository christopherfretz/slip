#include "side.h"
// §5.5 BaseSpec `access` (landed WP-027): a base written without an access
// specifier is private under `class`, public under `struct`. Both have a
// user-provided constructor, so that neither is an aggregate (a private base
// would change DefinitionBits `0-aggregate` too).
struct P {};
#ifdef B
class C : P { public: C() {} };
#else
struct C : P { C() {} };
#endif

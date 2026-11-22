#include "side.h"
// §5.5 DefinitionBits `0-dtor-for-call`.
#ifdef B
struct [[clang::trivial_abi]] T { ~T(); }; struct C { T t; };
#else
struct T { ~T(); }; struct C { T t; };
#endif

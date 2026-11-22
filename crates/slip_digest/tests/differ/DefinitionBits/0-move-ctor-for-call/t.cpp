#include "side.h"
// §5.5 DefinitionBits `0-move-ctor-for-call`.
#ifdef B
struct [[clang::trivial_abi]] T { T(T&&); ~T(); }; struct C { T t; };
#else
struct T { T(T&&); ~T(); }; struct C { T t; };
#endif

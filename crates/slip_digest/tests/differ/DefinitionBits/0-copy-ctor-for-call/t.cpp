#include "side.h"
// §5.5 DefinitionBits `0-copy-ctor-for-call`.
#ifdef B
struct [[clang::trivial_abi]] T { T(const T&); ~T(); }; struct C { T t; };
#else
struct T { T(const T&); ~T(); }; struct C { T t; };
#endif

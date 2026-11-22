#include "side.h"
// spec/digest.md §1 rule 2: qualified spelling does not matter.
#ifdef B
namespace n { struct S {}; n::S v; }
#else
namespace n { struct S {}; S v; }
#endif

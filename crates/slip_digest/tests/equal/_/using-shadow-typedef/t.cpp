#include "side.h"
// spec/digest.md §1 rule 2: using shadow typedef does not matter.
#ifdef B
namespace n { struct S {}; } using n::S; n::S v;
#else
namespace n { struct S {}; } using n::S; S v;
#endif

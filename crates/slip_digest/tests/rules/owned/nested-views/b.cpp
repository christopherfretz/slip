#include "dep.h"
namespace acme { namespace mid { namespace inner { struct S { int s; }; } } }
int user() { return sizeof(acme::mid::inner::S) + sizeof(acme::mid::D); }

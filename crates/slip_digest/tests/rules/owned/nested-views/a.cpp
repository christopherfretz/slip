#include "dep.h"
namespace legacy { namespace mid { namespace inner { struct S { int s; }; } } }
int user() { return sizeof(legacy::mid::inner::S) + sizeof(acme::mid::D); }

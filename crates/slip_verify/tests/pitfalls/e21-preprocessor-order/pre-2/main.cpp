
#pragma pack(push, 1)
#include "a.h"

#pragma pack(pop)
#include "b.h"

int sizes() { return sizeof(A) + sizeof(B); }

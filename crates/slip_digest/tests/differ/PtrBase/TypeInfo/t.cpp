#include "side.h"
// §8 PtrBase `TypeInfo`: `&typeid(int)` against a null pointer.
namespace std { class type_info; }
#ifdef B
constexpr const std::type_info* p = nullptr;
#else
constexpr const std::type_info* p = &typeid(int);
#endif

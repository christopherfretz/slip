#include "side.h"
// std: c++26
// §8 Scalar `Pointer.base`: a `const int*` to a variable (`Decl`) against one to a constant of the store (`Const`), same type, offset and designated type.
namespace std::meta {
using info = decltype(^^int);
template <class T> consteval T extract(info r);
template <class R> consteval info __kiln_reflect_constant_array(R&& r);
}
template <auto V> struct A;
const int g = 1;
constexpr int arr[] = {1};
#ifdef B
A<std::meta::extract<const int*>(std::meta::__kiln_reflect_constant_array(arr))>* x;
#else
A<&g>* x;
#endif

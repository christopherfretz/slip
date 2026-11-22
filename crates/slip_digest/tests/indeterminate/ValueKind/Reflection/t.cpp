// std: c++26
// §8 ValueKind `Reflection` (unsupported, §12): a reflection value as a template argument.
template <auto V> struct A;
A<^^int>* x;

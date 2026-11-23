// std: c++26
template <auto V> struct A;
int g(A<^^int> *);
int use() { return g(nullptr); }

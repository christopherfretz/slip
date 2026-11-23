// std: c++26
template <auto V> struct A;
int f(A<^^int> *);
int use() { return f(nullptr); }

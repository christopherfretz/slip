namespace n {
template <class T> struct Base { void f(); void f(int); };
template <class T> struct D : Base<T> { void f(); using Base<T>::f; };
}
void h(n::D<int>& d) { d.f(); d.f(1); }

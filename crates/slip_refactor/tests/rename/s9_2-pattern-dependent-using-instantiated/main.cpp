namespace n {
struct X { void f(int); };
template <class U> struct Base : X { void f(); };
template <class T> struct F : Base<T> { using Base<T>::f; };
}
void h(n::F<int>& x) { x.f(); }

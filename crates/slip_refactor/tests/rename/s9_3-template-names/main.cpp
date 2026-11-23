namespace n {
template <class T> void f(T) {}
}
void h() { n::f<int>(1); n::f(2.0); }

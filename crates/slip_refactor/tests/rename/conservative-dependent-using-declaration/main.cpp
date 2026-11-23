namespace n {
void f(int);
template <class B> struct D : B { using B::f; };
}
void h() { n::f(1); }

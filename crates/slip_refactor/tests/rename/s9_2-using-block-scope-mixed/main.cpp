namespace n {
void f(int);
inline namespace v { void f(double); }
}
void h() { using n::f; f(1); }

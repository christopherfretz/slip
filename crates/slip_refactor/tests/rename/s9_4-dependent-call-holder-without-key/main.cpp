namespace n {
void f(int);
template <class T> struct S { static_assert(sizeof(f(T())) > 0); };
}

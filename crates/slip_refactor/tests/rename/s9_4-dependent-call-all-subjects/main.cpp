namespace n {
void f(int);
void f(long);
template <class T> void t(T x) { f(x); }
}

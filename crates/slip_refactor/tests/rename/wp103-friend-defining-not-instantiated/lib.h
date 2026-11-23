namespace n {
void f(int);
template <class T> class A { friend void f(int) {} T x{}; };
inline void use() { f(1); }
}

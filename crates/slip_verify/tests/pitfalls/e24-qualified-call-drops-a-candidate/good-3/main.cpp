template <class T> struct Tr { static int r; };
template <class T> int Tr<T>::r = 0;
namespace lib {
struct X {};
template <class T> auto probe() { return (void)&Tr<T>::r, 1; }
template <class T, int = sizeof(probe<T>())> void f(T, char);
}
void f(lib::X, long) {}
void use() { ::f(lib::X(), 0); }
int main() { use(); }

namespace std_lib {
template <class T> T&& forward(T& t) noexcept;
template <class T> T&& forward(T&& t) noexcept;
}
namespace acme { using namespace std_lib; int g() { return 1; } }
template <class T> T f(T t) { return acme::forward<T>(t); }

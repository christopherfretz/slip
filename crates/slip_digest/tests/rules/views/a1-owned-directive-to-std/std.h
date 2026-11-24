namespace std {
struct string {};
template <class T> T&& forward(T& t) noexcept;
template <class T> T&& forward(T&& t) noexcept;
}

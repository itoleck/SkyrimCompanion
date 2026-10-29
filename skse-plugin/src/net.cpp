#include "net.h"

#define WIN32_LEAN_AND_MEAN
#include <WinSock2.h>
#include <WS2tcpip.h>

#include <algorithm>
#include <vector>

namespace companion::net
{
    void Serve(std::uint16_t port, std::chrono::milliseconds interval, const Callbacks& cb)
    {
        WSADATA wsa{};
        if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) {
            cb.error("WSAStartup failed");
            return;
        }

        SOCKET listener = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        sockaddr_in addr{};
        addr.sin_family = AF_INET;
        addr.sin_port = htons(port);
        inet_pton(AF_INET, "127.0.0.1", &addr.sin_addr);
        if (bind(listener, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) == SOCKET_ERROR ||
            listen(listener, SOMAXCONN) == SOCKET_ERROR) {
            cb.error("cannot listen on 127.0.0.1:" + std::to_string(port) + " (WSA error " + std::to_string(WSAGetLastError()) + ")");
            closesocket(listener);
            return;
        }
        cb.info("listening on 127.0.0.1:" + std::to_string(port));

        std::vector<SOCKET> clients;
        for (;;) {
            fd_set readable;
            FD_ZERO(&readable);
            FD_SET(listener, &readable);
            timeval timeout{ 0, static_cast<long>(std::chrono::microseconds(interval).count()) };
            if (select(0, &readable, nullptr, nullptr, &timeout) > 0 && FD_ISSET(listener, &readable)) {
                SOCKET client = accept(listener, nullptr, nullptr);
                if (client != INVALID_SOCKET) {
                    DWORD sendTimeoutMs = 200;
                    setsockopt(client, SOL_SOCKET, SO_SNDTIMEO, reinterpret_cast<const char*>(&sendTimeoutMs), sizeof(sendTimeoutMs));
                    BOOL noDelay = TRUE;
                    setsockopt(client, IPPROTO_TCP, TCP_NODELAY, reinterpret_cast<const char*>(&noDelay), sizeof(noDelay));
                    clients.push_back(client);
                    cb.info("client connected (" + std::to_string(clients.size()) + " total)");
                }
            }

            if (clients.empty()) {
                continue;
            }
            cb.onTick();

            const std::string line = cb.latest();
            if (line.empty()) {
                continue;
            }
            std::erase_if(clients, [&](SOCKET c) {
                if (send(c, line.data(), static_cast<int>(line.size()), 0) == SOCKET_ERROR) {
                    closesocket(c);
                    cb.info("client disconnected");
                    return true;
                }
                return false;
            });
        }
    }
}

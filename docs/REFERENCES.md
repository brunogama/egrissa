# Protocol References
- Apple Network `ProxyConfiguration`: supports relays, HTTP CONNECT, SOCKSv5 and defaults non-proxied failover to disabled.
- Apple `ProxyConfiguration.RelayHop`: secure relay hops proxy TCP with CONNECT and UDP with CONNECT-UDP; supports HTTP/3 with optional HTTP/2 fallback and one/two-hop configurations.
- RFC 9298: Proxying UDP in HTTP / CONNECT-UDP.
- RFC 9484: Proxying IP in HTTP / CONNECT-IP.

These references constrain adapter/protocol behavior; they do not define the Egress Runtime domain model.

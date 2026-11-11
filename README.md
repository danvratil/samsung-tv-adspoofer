# Samsung TV Ad Spoofer

Show custom photos and pictures in your Samsung TV SmartHub instead of adverts!

Thanks to the [eMudhra CA certificate fiasco](https://en.wikipedia.org/wiki/EMudhra#2026_weak_key_incident), we can create a fake implementation of the server that Samsung TVs (specifically the SmartHub) contact to fetch adverts. This simple service is intended to serve "ads" with user-specified images. Do you want to see pictures of your family or your pet, instead of random advertising on your SmartHub home screen? Now you can...

## Compatibility

I only have single Samsung TV I can test this on: `QE65Q60B - Q60B (2022) series`. If you were able to make this work with other Samsung TV, please let me know so I can update this section.

Also, as of today (2026-11-05) Samsung hasn't yet published a firmware update to disable the weak eMudhra CA keys. Once this happen, this tool will stop working. If you want to prevent that, and **if and only if you are fully aware of all the implications of such action**, you can consider disabling firmware updates for your TV to prevent Samsung for disabling the eMudhra certificate.

## Running

Getting this to work consist of two steps:

1) running the adspoofer service
2) redirecting the `*.adgrx.com` requests to the service

To run the AdSpoofer service, you need to supply a fake wildcard certificate for `*.adgrx.com` domain, signed by one of the "compromised" eMudhra CA certificates. Use your favourite search engine to find and generate such certificates.

Create a `certs/` directory and supply:

- `certs/fullchain.pem`: a PEM certificate and its intermediate chain, with the `*.adgrx.com` wildcard.
- `certs/privkey.pem`: the matching PEM private key

Then you can start the service:

```
cargo run --release
```

To force your TV to talk to the AdSpoofer instead of real `adgrx.co`m, you can use simple DNS redirection. If you are running Pihole, custom DNS server, or your router allows specifying custom DNS rules, just configure it to resolve `*.adgrx.com` to the IP address of the device where you are running the AdSpoofer.



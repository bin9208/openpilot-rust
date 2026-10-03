#include "usb_api.h"
#include "openpilot-usbgpu/src/usb_bridge.rs.h"
#include <limits>
namespace openpilot_usbgpu {
namespace {
struct TransferFree {
  std::shared_ptr<UsbApi> api;
  void operator()(libusb_transfer *transfer) const noexcept { if(transfer)api->free_transfer(transfer); }
};
struct Pending {
  std::unique_ptr<libusb_transfer,TransferFree> transfer;
  bool submitted=false;
  bool finished=false;
  explicit Pending(std::shared_ptr<UsbApi> api):transfer(api->alloc_transfer(0),{api}) {
    if(!transfer)throw std::bad_alloc();
  }
};
void LIBUSB_CALL completed(libusb_transfer *transfer) noexcept {
  static_cast<Pending *>(transfer->user_data)->finished=true;
}
}
void NativeUsb::batch(rust::Slice<UsbTransfer> requests) {
  if(requests.size()>124)throw std::invalid_argument("USB batch exceeds 31 four-transfer slots");
  std::vector<std::unique_ptr<Pending>> pending;
  pending.reserve(requests.size());
  for(auto &request:requests) {
    if(request.data.size()>std::numeric_limits<int>::max())throw std::invalid_argument("USB stream buffer size");
    auto item=std::make_unique<Pending>(impl_->api);
    auto *transfer=item->transfer.get();
    transfer->dev_handle=impl_->handle.get();transfer->endpoint=request.endpoint;
    transfer->type=request.use_stream?LIBUSB_TRANSFER_TYPE_BULK_STREAM:LIBUSB_TRANSFER_TYPE_BULK;
    transfer->timeout=request.timeout_ms;transfer->buffer=request.data.data();
    transfer->length=static_cast<int>(request.data.size());transfer->callback=completed;transfer->user_data=item.get();
    if(request.use_stream)impl_->api->transfer_set_stream_id(transfer,request.stream);
    pending.push_back(std::move(item));
  }
  int failure=0;
  const char *operation="libusb_submit_transfer";
  for(auto &item:pending) {
    const int status=impl_->api->submit_transfer(item->transfer.get());
    if(status<0) {failure=status;break;}
    item->submitted=true;
  }
  bool cancelled=false;
  for(;;) {
    bool active=false;
    for(auto &item:pending)active|=item->submitted && !item->finished;
    if(!active)break;
    if(failure && !cancelled) {
      for(auto &item:pending)if(item->submitted && !item->finished) {
        const int status=impl_->api->cancel_transfer(item->transfer.get());
        if(status<0 && status!=LIBUSB_ERROR_NOT_FOUND)std::fputs("usbgpu: USB cancellation failed; awaiting terminal callback\n",stderr);
      }
      cancelled=true;
    }
    timeval interval{0,10000};
    const int status=impl_->api->handle_events_timeout_completed(impl_->context.get(),&interval,nullptr);
    if(status<0 && status!=LIBUSB_ERROR_INTERRUPTED && !failure) {failure=status;operation="libusb_handle_events";}
  }
  impl_->api->checked(failure,operation);
  for(std::size_t i=0;i<requests.size();++i) {
    auto *transfer=pending[i]->transfer.get();
    if(transfer->actual_length<0 || transfer->actual_length>transfer->length)throw std::runtime_error("USB stream length outside buffer");
    requests[i].status=static_cast<std::int32_t>(transfer->status);
    requests[i].actual=static_cast<std::uint32_t>(transfer->actual_length);
  }
}
}

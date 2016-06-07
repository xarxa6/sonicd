package build.unstable.sonicd.system

import akka.actor.SupervisorStrategy.Stop
import akka.actor._
import akka.stream.actor.{ActorPublisher, ActorSubscriber, OneByOneRequestStrategy, RequestStrategy}
import build.unstable.sonicd.model.Exceptions.ProtocolException
import build.unstable.sonicd.model._
import build.unstable.sonicd.system.SonicController.NewQuery
import org.reactivestreams._

class WsHandler(controller: ActorRef) extends ActorPublisher[SonicMessage]
with ActorSubscriber with ActorLogging {

  import akka.stream.actor.ActorPublisherMessage._
  import akka.stream.actor.ActorSubscriberMessage._

  override protected def requestStrategy: RequestStrategy = OneByOneRequestStrategy

  case object CompletedStream

  override def supervisorStrategy: SupervisorStrategy = OneForOneStrategy() {
    case e: Exception ⇒
      log.error(e, "error in publisher")
      self ! DoneWithQueryExecution.error(e)
      Stop
  }

  def awaitingAck: Receive = {
    case Cancel | OnComplete | OnNext(ClientAcknowledge) ⇒ onCompleteThenStop()
  }

  def closing(done: DoneWithQueryExecution): Receive = {
    log.debug("switched to closing behaviour with ev {}", done)
    if (isActive && totalDemand > 0) {
      onNext(done)
      context.become(awaitingAck)
    }

    {
      case Cancel | OnComplete | OnNext(ClientAcknowledge) ⇒ onCompleteThenStop()
      case Request(n) =>
        if (isActive) {
          onNext(done)
          context.become(awaitingAck)
        }
    }
  }

  val subs = new Subscriber[SonicMessage] {

    override def onError(t: Throwable): Unit = {
      log.error(t, "publisher called onError of wsHandler")
      self ! DoneWithQueryExecution.error(t)
    }

    override def onSubscribe(s: Subscription): Unit = self ! s

    override def onComplete(): Unit = self ! CompletedStream

    override def onNext(t: SonicMessage): Unit = self ! t

  }

  def commonBehaviour: Receive = {

    case Cancel | OnComplete | OnNext(ClientAcknowledge) ⇒
      log.debug("client completed/canceled stream")
      onCompleteThenStop()

    case CompletedStream ⇒
      val msg = "completed stream without done msg"
      log.error(msg)
      context.become(closing(DoneWithQueryExecution.error(new ProtocolException(msg))))

    case msg@OnError(e) ⇒
      log.error(e, "error in ws stream")
      context.become(closing(DoneWithQueryExecution.error(e)))

    case msg: DoneWithQueryExecution ⇒
      context.become(closing(msg))

  }

  var pendingToStream: Long = 0L

  def requestTil(implicit s: Subscription): Unit = {
    //make sure that requested is never > than totalDemand
    while (pendingToStream < totalDemand) {
      s.request(1)
      pendingToStream += 1L
    }
  }

  def materialized(subscription: Subscription): Receive = {
    val recv: Receive = {

      case msg@Request(n) ⇒ requestTil(subscription)

      case msg: DoneWithQueryExecution ⇒ context.become(closing(msg))

      case a@(OnComplete | Cancel | OnNext(ClientAcknowledge)) ⇒
        commonBehaviour.apply(a)
        subscription.cancel() //cancel source

      case msg: SonicMessage ⇒
        try {
          if (isActive) {
            onNext(msg)
            pendingToStream -= 1L
          }
          else log.warning(s"dropping message $msg: wsHandler is not active")
        } catch {
          case e: Exception ⇒
            log.error(e, s"error onNext: pending: $pendingToStream; demand: $totalDemand")
            context.become(closing(DoneWithQueryExecution.error(e)))
        }
    }
    recv orElse commonBehaviour
  }

  def initial: Receive = {

    case s: Subscription ⇒
      requestTil(s)
      context.become(materialized(s))

    case handlerProps: Props ⇒
      log.debug("received props {}", handlerProps)
      val handler = context.actorOf(handlerProps)
      val pub = ActorPublisher[SonicMessage](handler)
      pub.subscribe(subs)

    case OnNext(msg) ⇒
      log.debug("client established communication with ws handler")
      try {
        val query = msg.asInstanceOf[Query]
        controller ! NewQuery(query)
      } catch {
        case e: Exception ⇒
          log.error(e, "error while processing query")
          context.become(closing(DoneWithQueryExecution.error(e)))
      }
  }

  override def receive: Receive = initial orElse commonBehaviour
}
